(define list (lambda xs (reverse xs)))

(define (fold f start xs)
    (if (null? xs)
        start
        (fold f (f (car xs) start) (cdr xs))))

(define (reverse xs) (fold cons '() xs))

(define (map f lst)
    (if (null? lst)
        '()
        (cons (f (car lst)) (map f (cdr lst)))))

(define (zip as bs)
    (if (or (null? as) (null? bs))
        '()
        (cons (cons (car as) (car bs)) (zip (cdr as) (cdr bs)))))

(define (for-each f lst)
    (map f lst)
    '())

(define (filter p lst)
    (fold
        (lambda (x acc)
            (if (p x)
                (cons x acc)
                acc))
        '()
        lst))

(define (not x) (if x #f #t))

; technically the specs say it has to be short circuiting but i dont care
(define (or a b)
    (if a
        #t
        (if b
            #t
            #f)))

(define (and a b)
    (if a
        (if b
            #t
            #f)
        #f))

(define (>= a b) (or (> a b) (= a b)))
(define (<= a b) (or (< a b) (= a b)))

(define (square x) (* x x))

(define (derivative f)
    (define epsilon 0.0001)
    (lambda (x)
        (let ((low (f (- x epsilon))) (high (f (+ x epsilon))))
            (/ (- high low) (+ epsilon epsilon)))))

(define (sqrt x) x)
