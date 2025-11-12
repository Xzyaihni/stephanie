(define list (lambda xs xs))

(define else #t)

(define newline-char #\
)

(define (equal? a b)
    (cond
        ((and (null? a) (pair? b)) #f)
        ((and (null? b) (pair? a)) #f)
        ((and (pair? a) (pair? b))
            (if (= (car a) (car b))
                (equal? (cdr a) (cdr b))
                #f))
        (else (eq? a b))))

(define (list-tail xs n)
    (if (= n 0)
        xs
        (list-tail (cdr xs) (- n 1))))

(define (list-ref xs n) (car (list-tail xs n)))

(define (list-remove xs x)
    (if (null? x)
        xs
        (filter (lambda (v) (not (equal? x v))) xs)))

(define (list->vector xs)
    (define v (make-vector (length xs) #\ ))
    (define (inner current i)
        (if (not (null? current))
            (begin
                (vector-set! v i (car current))
                (inner (cdr current) (+ i 1)))))
    (inner xs 0)
    v)

(define (vector->list xs)
    (define len (vector-length xs))
    (define (inner i)
        (if (= i len)
            '()
            (cons (vector-ref xs i) (inner (+ i 1)))))
    (inner 0))

(define (append as bs)
    (fold cons bs (reverse as)))

(define (counter x)
    (define (counter-inner current)
        (if (< current x)
            (cons current (counter-inner (+ current 1)))
            '()))
    (counter-inner 0))

(define (range start end)
    (map (lambda (x) (+ x start)) (counter (- end start))))

(define (fold f start xs)
    (if (null? xs)
        start
        (fold f (f (car xs) start) (cdr xs))))

(define (fold1 f xs)
    (fold f (car xs) (cdr xs)))

(define (reverse xs) (fold cons '() xs))

(define (map f xs)
    (if (null? xs)
        '()
        (cons (f (car xs)) (map f (cdr xs)))))

(define (zip as bs)
    (if (or (null? as) (null? bs))
        '()
        (cons (cons (car as) (car bs)) (zip (cdr as) (cdr bs)))))

(define (find f xs)
    (if (null? xs)
        '()
        (if (f (car xs))
            (car xs)
            (find f (cdr xs)))))

(define (find-index f xs)
    (if (null? xs)
        '()
        (if (f (car xs))
            0
            (+ 1 (find-index f (cdr xs))))))

(define (for-each f xs)
    (if (null? xs)
        '()
        (begin
            (f (car xs))
            (for-each f (cdr xs)))))

(define (filter p xs)
    (if (null? xs)
        '()
        (if (p (car xs))
            (cons (car xs) (filter p (cdr xs)))
            (filter p (cdr xs)))))

(define (all xs)
    (cond
        ((null? xs) #t)
        ((car xs) (all (cdr xs)))
        (else #f)))

(define (any xs)
    (cond
        ((null? xs) #f)
        ((car xs) #t)
        (else (any (cdr xs)))))

; does a bubble sort
(define (sort id xs)
    (define (is-sorted xs)
        (if (or (null? xs) (null? (cdr xs)))
            #t
            (if (> (id (car xs)) (id (car (cdr xs))))
                #f
                (is-sorted (cdr xs)))))
    (define (swap-one xs)
        (if (null? (cdr xs))
            xs
            (let ((a (car xs)) (b (car (cdr xs))))
                (if (> (id a) (id b))
                    (cons b (cons a (cdr (cdr xs))))
                    (cons a (swap-one (cdr xs)))))))
    (define (do-once xs)
        (if (is-sorted xs)
            xs
            (do-once (swap-one xs))))
    (do-once xs))


(define (drop xs n)
    (if (or (= n 0) (null? xs))
        xs
        (drop (cdr xs) (- n 1))))

(define (take xs n)
    (if (or (= n 0) (null? xs))
        '()
        (cons (car xs) (take (cdr xs) (- n 1)))))

(define (replicate n x)
    (if (= n 0)
        '()
        (cons x (replicate (- n 1) x))))

(define (repeat f n)
    (if (= n 0)
        '()
        (begin (f) (repeat f (- n 1)))))

(define (length xs)
    (fold (lambda (_ acc) (+ acc 1)) 0 xs))

(define (not x) (if x #f #t))

; technically the specs say it has to be short circuiting but i dont care
(define (or a b)
    (if a #t b))

(define (and a b)
    (if a b #f))

(define (xor a b)
    (if a (not b) b))

(define (>= a b) (or (> a b) (= a b)))
(define (<= a b) (or (< a b) (= a b)))

(define (abs x)
    (if (< x 0.0)
        (- 0 x)
        x))

(define (square x) (* x x))

(define (expi x p)
    (if (= p 0)
        1
        (if (= p 1)
            x
            (* x (expi x (- p 1))))))

(define (factorial x)
    (if (<= x 0)
        1
        (* x (factorial (- x 1)))))

(define (derivative f)
    (define epsilon 0.0001)
    (lambda (x)
        (let ((low (f (- x epsilon))) (high (f (+ x epsilon))))
            (/ (- high low) (+ epsilon epsilon)))))

(define (newtons-method initial f)
    (define df (derivative f))
    (define (newtons-method-inner x index)
        (let ((current (f x)))
            (if (> index 10000)
                x
                (if (< (abs current) 0.00001)
                    x
                    (newtons-method-inner
                        (- x (/ current (df x)))
                        (+ index 1))))))
    (newtons-method-inner initial 0))

(define (expm1 x)
    (define exp-iterations 10)
    (define (expm1-inner i sum)
        (if (> i exp-iterations)
            sum
            (expm1-inner
                (+ i 1.0)
                (+ (/ (expi x i) (factorial i)) sum))))
    (if (= x 0)
        0.0
        (expm1-inner 2.0 (exact->inexact x))))

(define (exp x)
    (+ (expm1 x) 1))

(define (ln x)
    (newtons-method x (lambda (y) (- (exp y) x))))

(define (expt b x)
    (exp (* x (ln b))))

(define (sqrt x)
    (newtons-method 1.0 (lambda (y) (- (* y y) x))))

(define (random-choice xs)
    (if (null? xs)
        '()
        (list-ref xs (random-integer-between 0 (length xs)))))

; start inclusive, end exclusive
(define (random-integer-between start end)
    (let ((distance (- end start)))
        (+ start (random-integer distance))))

(define (random-bool)
    (= (random-integer 2) 1))
