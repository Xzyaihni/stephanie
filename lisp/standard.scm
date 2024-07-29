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

(define (for-each f lst)
    (map f lst)
    '())

; technically the specs say it has to be short circuiting but i dont care
(define (or a b)
    (if a
        #t
        (if b
            #t
            #f)))
